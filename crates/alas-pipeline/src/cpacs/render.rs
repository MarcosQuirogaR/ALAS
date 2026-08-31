// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS XML rendering and source-geometry validation.

#[path = "analysis.rs"]
mod analysis;
mod engine;

#[cfg(test)]
// Renderer fixtures intentionally use expect so malformed test setup is reported
// at the fixture boundary instead of being converted into production fallbacks.
#[allow(clippy::expect_used)]
mod render_tests;

use std::fmt::Write as FmtWrite;

use alas_config::ActiveEngineModel;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;
use alas_mission::MissionResult;

use super::{
    CpacsExportError, AIRCRAFT_MODEL_UID, CPACS_V35_SCHEMA_URL, ENGINE_UID, FUSELAGE_PROFILE_UID,
};
use crate::feasibility::FeasibilityReport;
use crate::full_analysis::AnalysisReport;

/// Render a deterministic CPACS 3.5 document for `airplane`.
///
/// `timestamp` is explicit to make schema and geometry tests reproducible.
/// Production callers should use [`super::export_cpacs`], which supplies a
/// current UTC timestamp.
pub fn render_cpacs_v35(
    airplane: &Airplane,
    config: &AlasConfig,
    timestamp: &str,
) -> Result<String, CpacsExportError> {
    validate_airplane(airplane, config)?;

    render_document(airplane, config, timestamp, None, None, None)
}

/// Render a CPACS 3.5 document with the analyses already available to a caller.
///
/// The geometry-only [`render_cpacs_v35`] API remains compatible for callers
/// that do not have a completed analysis report. Optional feasibility and
/// mission records are emitted only through CPACS structures that can represent
/// their available values without inventing missing quantities.
pub(super) fn render_cpacs_v35_with_analysis(
    report: &AnalysisReport,
    config: &AlasConfig,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
    timestamp: &str,
) -> Result<String, CpacsExportError> {
    validate_airplane(&report.airplane, config)?;

    render_document(
        &report.airplane,
        config,
        timestamp,
        Some(report),
        feasibility,
        mission,
    )
}

fn render_document(
    airplane: &Airplane,
    config: &AlasConfig,
    timestamp: &str,
    report: Option<&AnalysisReport>,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
) -> Result<String, CpacsExportError> {
    let mut xml = String::with_capacity(96_000);
    let _ = writeln!(xml, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        xml,
        "<cpacs xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"{CPACS_V35_SCHEMA_URL}\">"
    );
    write_header(&mut xml, airplane, timestamp);
    write_vehicles(&mut xml, airplane, config, report, feasibility, mission)?;
    engine::write_toolspecific_propulsion(&mut xml, config);
    let _ = writeln!(xml, "</cpacs>");
    Ok(xml)
}

fn validate_airplane(airplane: &Airplane, config: &AlasConfig) -> Result<(), CpacsExportError> {
    if airplane.wings.is_empty() {
        return Err(CpacsExportError::NoWings);
    }
    validate_number(airplane.s_ref, "airplane reference area")?;
    validate_number(airplane.c_ref, "airplane reference chord")?;
    validate_number(airplane.b_ref, "airplane reference span")?;
    validate_vector(airplane.xyz_ref, "airplane reference point")?;
    let active_engine = config
        .geometry
        .engine
        .active_model()
        .map_err(|error| CpacsExportError::InvalidEngineBinding(error.to_string()))?;
    if let ActiveEngineModel::Turbofan(spec) = active_engine {
        validate_number(spec.rated_thrust_kn, "engine take-off thrust")?;
        validate_number(spec.bypass_ratio, "engine bypass ratio")?;
        validate_number(spec.overall_pressure_ratio, "engine overall pressure ratio")?;
        validate_number(spec.fan_pressure_ratio, "engine fan pressure ratio")?;
    }
    validate_number(
        config.geometry.engine.radius_scale_m,
        "engine maximum nacelle radius",
    )?;
    validate_number(
        config.geometry.engine.nacelle_length_m(),
        "engine nacelle length",
    )?;
    for wing in &airplane.wings {
        if wing.xsecs.len() < 2 {
            return Err(CpacsExportError::TooFewWingSections {
                name: wing.name.clone(),
                count: wing.xsecs.len(),
            });
        }
        for (index, section) in wing.xsecs.iter().enumerate() {
            validate_vector(
                section.xyz_le,
                &format!("wing {:?} section {index} leading edge", wing.name),
            )?;
            validate_number(
                section.chord,
                &format!("wing {:?} section {index} chord", wing.name),
            )?;
            validate_number(
                section.twist,
                &format!("wing {:?} section {index} twist", wing.name),
            )?;
            if section.airfoil.coordinates.len() < 3 {
                return Err(CpacsExportError::TooFewAirfoilPoints {
                    name: section.airfoil.name.clone(),
                    count: section.airfoil.coordinates.len(),
                });
            }
            for (point_index, &(x, z)) in section.airfoil.coordinates.iter().enumerate() {
                validate_number(
                    x,
                    &format!(
                        "wing {:?} section {index} airfoil point {point_index} x",
                        wing.name
                    ),
                )?;
                validate_number(
                    z,
                    &format!(
                        "wing {:?} section {index} airfoil point {point_index} z",
                        wing.name
                    ),
                )?;
            }
        }
    }

    for fuselage in &airplane.fuselages {
        if fuselage.xsecs.len() < 2 {
            return Err(CpacsExportError::TooFewFuselageSections {
                name: fuselage.name.clone(),
                count: fuselage.xsecs.len(),
            });
        }
        for (index, section) in fuselage.xsecs.iter().enumerate() {
            validate_vector(
                section.xyz_c,
                &format!("fuselage {:?} section {index} centre", fuselage.name),
            )?;
            validate_number(
                section.width,
                &format!("fuselage {:?} section {index} width", fuselage.name),
            )?;
            validate_number(
                section.height,
                &format!("fuselage {:?} section {index} height", fuselage.name),
            )?;
        }
    }

    Ok(())
}

fn validate_number(value: f64, context: &str) -> Result<(), CpacsExportError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CpacsExportError::NonFinite {
            context: context.to_owned(),
        })
    }
}

fn validate_vector(vector: [f64; 3], context: &str) -> Result<(), CpacsExportError> {
    for (axis, value) in ["x", "y", "z"].into_iter().zip(vector) {
        validate_number(value, &format!("{context} {axis}"))?;
    }
    Ok(())
}

fn write_header(xml: &mut String, airplane: &Airplane, timestamp: &str) {
    let _ = writeln!(xml, "  <header>");
    write_text(
        xml,
        4,
        "name",
        &format!("{} CPACS interchange", airplane.name),
    );
    write_text(
        xml,
        4,
        "description",
        "Computed aircraft geometry and reference data exported by ALAS.",
    );
    write_text(xml, 4, "version", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(xml, "    <versionInfos>");
    let _ = writeln!(
        xml,
        "      <versionInfo version=\"{}\">",
        env!("CARGO_PKG_VERSION")
    );
    write_text(xml, 8, "cpacsVersion", "3.5");
    write_text(
        xml,
        8,
        "description",
        "Initial CPACS 3.5 aircraft dataset generated by ALAS.",
    );
    write_text(xml, 8, "timestamp", timestamp);
    write_text(xml, 8, "creator", "ALAS");
    let _ = writeln!(xml, "      </versionInfo>");
    let _ = writeln!(xml, "    </versionInfos>");
    let _ = writeln!(xml, "  </header>");
}

fn write_vehicles(
    xml: &mut String,
    airplane: &Airplane,
    config: &AlasConfig,
    report: Option<&AnalysisReport>,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
) -> Result<(), CpacsExportError> {
    let _ = writeln!(xml, "  <vehicles>");
    write_aircraft(xml, airplane, report, config, feasibility, mission)?;
    engine::write_engine_definition(xml, config);
    write_profiles(xml, airplane);
    let _ = writeln!(xml, "  </vehicles>");
    Ok(())
}

fn write_aircraft(
    xml: &mut String,
    airplane: &Airplane,
    report: Option<&AnalysisReport>,
    config: &AlasConfig,
    feasibility: Option<&FeasibilityReport>,
    mission: Option<&MissionResult>,
) -> Result<(), CpacsExportError> {
    let _ = writeln!(xml, "    <aircraft>");
    let _ = writeln!(xml, "      <model uID=\"{AIRCRAFT_MODEL_UID}\">");
    write_text(xml, 8, "name", &airplane.name);
    write_text(
        xml,
        8,
        "description",
        "Computed outer geometry exported by ALAS-rust; no unmodelled internal geometry is implied.",
    );
    write_reference(xml, airplane);
    if !airplane.fuselages.is_empty() {
        write_fuselages(xml, airplane);
    }
    write_wings(xml, airplane, !airplane.fuselages.is_empty())?;
    write_engine_positions(xml, airplane);
    if let Some(report) = report {
        analysis::write_aircraft_analyses(xml, report, config, feasibility, mission);
    }
    let _ = writeln!(xml, "      </model>");
    let _ = writeln!(xml, "    </aircraft>");
    Ok(())
}

fn write_reference(xml: &mut String, airplane: &Airplane) {
    let _ = writeln!(xml, "        <reference>");
    write_number_element(xml, 10, "area", airplane.s_ref);
    write_number_element(xml, 10, "length", airplane.c_ref);
    write_point(xml, 10, "point", airplane.xyz_ref, None);
    let _ = writeln!(xml, "        </reference>");
}

fn write_fuselages(xml: &mut String, airplane: &Airplane) {
    let _ = writeln!(xml, "        <fuselages>");
    for (fuselage_index, fuselage) in airplane.fuselages.iter().enumerate() {
        let uid = fuselage_uid(fuselage_index);
        let _ = writeln!(xml, "          <fuselage uID=\"{uid}\">");
        write_text(xml, 12, "name", &fuselage.name);
        write_transformation(xml, 12, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let _ = writeln!(xml, "            <sections>");
        for (section_index, section) in fuselage.xsecs.iter().enumerate() {
            let section_uid = fuselage_section_uid(fuselage_index, section_index);
            let element_uid = fuselage_element_uid(fuselage_index, section_index);
            let _ = writeln!(xml, "              <section uID=\"{section_uid}\">");
            write_text(
                xml,
                16,
                "name",
                &format!("{} section {section_index}", fuselage.name),
            );
            write_transformation(xml, 16, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], section.xyz_c);
            let _ = writeln!(xml, "                <elements>");
            let _ = writeln!(xml, "                  <element uID=\"{element_uid}\">");
            write_text(
                xml,
                20,
                "name",
                &format!("{} profile {section_index}", fuselage.name),
            );
            write_text(xml, 20, "profileUID", FUSELAGE_PROFILE_UID);
            write_transformation(
                xml,
                20,
                [1.0, section.width / 2.0, section.height / 2.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            );
            let _ = writeln!(xml, "                  </element>");
            let _ = writeln!(xml, "                </elements>");
            let _ = writeln!(xml, "              </section>");
        }
        let _ = writeln!(xml, "            </sections>");
        let _ = writeln!(xml, "            <segments>");
        for section_index in 1..fuselage.xsecs.len() {
            let segment_uid = fuselage_segment_uid(fuselage_index, section_index - 1);
            let _ = writeln!(xml, "              <segment uID=\"{segment_uid}\">");
            write_text(
                xml,
                16,
                "name",
                &format!("{} segment {}", fuselage.name, section_index - 1),
            );
            write_text(
                xml,
                16,
                "fromElementUID",
                &fuselage_element_uid(fuselage_index, section_index - 1),
            );
            write_text(
                xml,
                16,
                "toElementUID",
                &fuselage_element_uid(fuselage_index, section_index),
            );
            let _ = writeln!(xml, "              </segment>");
        }
        let _ = writeln!(xml, "            </segments>");
        let _ = writeln!(xml, "          </fuselage>");
    }
    let _ = writeln!(xml, "        </fuselages>");
}

fn write_wings(
    xml: &mut String,
    airplane: &Airplane,
    has_fuselage: bool,
) -> Result<(), CpacsExportError> {
    let _ = writeln!(xml, "        <wings>");
    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        write_wing(xml, wing_index, wing, has_fuselage)?;
    }
    let _ = writeln!(xml, "        </wings>");
    Ok(())
}

fn write_wing(
    xml: &mut String,
    wing_index: usize,
    wing: &Wing,
    has_fuselage: bool,
) -> Result<(), CpacsExportError> {
    let uid = wing_uid(wing_index);
    let symmetry = if wing.symmetric {
        " symmetry=\"x-z-plane\""
    } else {
        ""
    };
    let _ = writeln!(xml, "          <wing uID=\"{uid}\"{symmetry}>");
    write_text(xml, 12, "name", &wing.name);
    if wing_index > 0 && has_fuselage {
        write_text(xml, 12, "parentUID", &fuselage_uid(0));
    }
    let root = wing.xsecs[0].xyz_le;
    write_transformation(xml, 12, [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], root);
    let _ = writeln!(xml, "            <sections>");
    for (section_index, section) in wing.xsecs.iter().enumerate() {
        let section_uid = wing_section_uid(wing_index, section_index);
        let element_uid = wing_element_uid(wing_index, section_index);
        let dihedral = section_dihedral_deg(wing, section_index)?;
        let translation = subtract(section.xyz_le, root);
        let _ = writeln!(xml, "              <section uID=\"{section_uid}\">");
        write_text(
            xml,
            16,
            "name",
            &format!("{} section {section_index}", wing.name),
        );
        write_transformation(xml, 16, [1.0, 1.0, 1.0], [dihedral, 0.0, 0.0], translation);
        let _ = writeln!(xml, "                <elements>");
        let _ = writeln!(xml, "                  <element uID=\"{element_uid}\">");
        write_text(
            xml,
            20,
            "name",
            &format!("{} airfoil {section_index}", wing.name),
        );
        write_text(
            xml,
            20,
            "airfoilUID",
            &wing_airfoil_uid(wing_index, section_index),
        );
        write_transformation(
            xml,
            20,
            [section.chord, 1.0, section.chord],
            [0.0, section.twist, 0.0],
            [0.0, 0.0, 0.0],
        );
        let _ = writeln!(xml, "                  </element>");
        let _ = writeln!(xml, "                </elements>");
        let _ = writeln!(xml, "              </section>");
    }
    let _ = writeln!(xml, "            </sections>");
    let _ = writeln!(xml, "            <segments>");
    for section_index in 1..wing.xsecs.len() {
        let _ = writeln!(
            xml,
            "              <segment uID=\"{}\">",
            wing_segment_uid(wing_index, section_index - 1)
        );
        write_text(
            xml,
            16,
            "name",
            &format!("{} segment {}", wing.name, section_index - 1),
        );
        write_text(
            xml,
            16,
            "fromElementUID",
            &wing_element_uid(wing_index, section_index - 1),
        );
        write_text(
            xml,
            16,
            "toElementUID",
            &wing_element_uid(wing_index, section_index),
        );
        let _ = writeln!(xml, "              </segment>");
    }
    let _ = writeln!(xml, "            </segments>");
    let _ = writeln!(xml, "          </wing>");
    Ok(())
}

fn write_engine_positions(xml: &mut String, airplane: &Airplane) {
    let nacelles: Vec<&Fuselage> = airplane
        .fuselages
        .iter()
        .filter(|fuselage| is_nacelle(fuselage))
        .collect();
    if nacelles.is_empty() {
        return;
    }

    let main_fuselage = airplane
        .fuselages
        .iter()
        .position(|fuselage| !is_nacelle(fuselage));
    let parent_uid = main_fuselage
        .map(fuselage_uid)
        .unwrap_or_else(|| wing_uid(0));
    let parent_origin = main_fuselage.map_or(airplane.wings[0].xsecs[0].xyz_le, |_| [0.0; 3]);

    let _ = writeln!(xml, "        <engines>");
    for (position_index, nacelle) in nacelles.into_iter().enumerate() {
        let center = nacelle.xsecs[0].xyz_c;
        let _ = writeln!(
            xml,
            "          <engine uID=\"alas-engine-position-{position_index}\">"
        );
        write_text(xml, 12, "name", &nacelle.name);
        write_text(xml, 12, "engineUID", ENGINE_UID);
        write_text(xml, 12, "parentUID", &parent_uid);
        write_transformation(
            xml,
            12,
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            subtract(center, parent_origin),
        );
        let _ = writeln!(xml, "          </engine>");
    }
    let _ = writeln!(xml, "        </engines>");
}

fn write_profiles(xml: &mut String, airplane: &Airplane) {
    let _ = writeln!(xml, "    <profiles>");
    let _ = writeln!(xml, "      <fuselageProfiles>");
    let _ = writeln!(
        xml,
        "        <fuselageProfile uID=\"{FUSELAGE_PROFILE_UID}\">"
    );
    write_text(xml, 10, "name", "Unit ellipse");
    write_text(
        xml,
        10,
        "description",
        "Unit fuselage profile scaled by each source section width and height.",
    );
    write_unit_ellipse(xml, 10);
    let _ = writeln!(xml, "        </fuselageProfile>");
    let _ = writeln!(xml, "      </fuselageProfiles>");
    let _ = writeln!(xml, "      <wingAirfoils>");
    for (wing_index, wing) in airplane.wings.iter().enumerate() {
        for (section_index, section) in wing.xsecs.iter().enumerate() {
            let uid = wing_airfoil_uid(wing_index, section_index);
            let _ = writeln!(xml, "        <wingAirfoil uID=\"{uid}\">");
            write_text(xml, 10, "name", &section.airfoil.name);
            write_text(
                xml,
                10,
                "description",
                &format!(
                    "{} section {section_index}; source contour is retained exactly.",
                    wing.name
                ),
            );
            write_airfoil_points(xml, &section.airfoil.coordinates);
            let _ = writeln!(xml, "        </wingAirfoil>");
        }
    }
    let _ = writeln!(xml, "      </wingAirfoils>");
    let _ = writeln!(xml, "    </profiles>");
}

fn write_transformation(
    xml: &mut String,
    indent: usize,
    scaling: [f64; 3],
    rotation: [f64; 3],
    translation: [f64; 3],
) {
    let pad = " ".repeat(indent);
    let _ = writeln!(xml, "{pad}<transformation>");
    write_point(xml, indent + 2, "scaling", scaling, None);
    write_point(xml, indent + 2, "rotation", rotation, None);
    write_point(
        xml,
        indent + 2,
        "translation",
        translation,
        Some("absLocal"),
    );
    let _ = writeln!(xml, "{pad}</transformation>");
}

fn write_point(
    xml: &mut String,
    indent: usize,
    element: &str,
    point: [f64; 3],
    ref_type: Option<&str>,
) {
    let pad = " ".repeat(indent);
    let ref_attribute = ref_type.map_or_else(String::new, |value| format!(" refType=\"{value}\""));
    let _ = writeln!(xml, "{pad}<{element}{ref_attribute}>");
    write_number_element(xml, indent + 2, "x", point[0]);
    write_number_element(xml, indent + 2, "y", point[1]);
    write_number_element(xml, indent + 2, "z", point[2]);
    let _ = writeln!(xml, "{pad}</{element}>");
}

fn write_unit_ellipse(xml: &mut String, indent: usize) {
    let points = (0..=16).map(|index| {
        let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * index as f64 / 16.0;
        [0.0, angle.cos(), angle.sin()]
    });
    write_point_vectors(xml, indent, points);
}

fn write_airfoil_points(xml: &mut String, coordinates: &[(f64, f64)]) {
    // CPACS orders an airfoil from lower trailing edge, around the leading
    // edge, to upper trailing edge. ALAS stores the opposite conventional
    // Selig order, so reversing preserves every original coordinate.
    write_point_vectors(xml, 10, coordinates.iter().rev().map(|&(x, z)| [x, 0.0, z]));
}

fn write_point_vectors<I>(xml: &mut String, indent: usize, points: I)
where
    I: IntoIterator<Item = [f64; 3]>,
{
    let points: Vec<[f64; 3]> = points.into_iter().collect();
    let pad = " ".repeat(indent);
    let _ = writeln!(xml, "{pad}<pointList>");
    write_vector(
        xml,
        indent + 2,
        "x",
        &join_coordinate(points.iter().map(|point| point[0])),
    );
    write_vector(
        xml,
        indent + 2,
        "y",
        &join_coordinate(points.iter().map(|point| point[1])),
    );
    write_vector(
        xml,
        indent + 2,
        "z",
        &join_coordinate(points.iter().map(|point| point[2])),
    );
    let _ = writeln!(xml, "{pad}</pointList>");
}

fn join_coordinate(values: impl Iterator<Item = f64>) -> String {
    values.map(number).collect::<Vec<_>>().join(";")
}

fn write_text(xml: &mut String, indent: usize, element: &str, value: &str) {
    let pad = " ".repeat(indent);
    let _ = writeln!(xml, "{pad}<{element}>{}</{element}>", xml_escape(value));
}

fn write_number_element(xml: &mut String, indent: usize, element: &str, value: f64) {
    let pad = " ".repeat(indent);
    let _ = writeln!(xml, "{pad}<{element}>{}</{element}>", number(value));
}

fn write_vector(xml: &mut String, indent: usize, element: &str, value: &str) {
    let pad = " ".repeat(indent);
    let _ = writeln!(
        xml,
        "{pad}<{element} mapType=\"vector\">{}</{element}>",
        xml_escape(value)
    );
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}

fn number(value: f64) -> String {
    value.to_string()
}

fn section_dihedral_deg(wing: &Wing, section_index: usize) -> Result<f64, CpacsExportError> {
    let current = wing.xsecs[section_index].xyz_le;
    let failure = || CpacsExportError::DegenerateWingStation {
        name: wing.name.clone(),
        section: section_index,
    };
    let direction = if section_index == 0 {
        subtract(wing.xsecs[1].xyz_le, current)
    } else if section_index + 1 == wing.xsecs.len() {
        subtract(current, wing.xsecs[section_index - 1].xyz_le)
    } else {
        let before = normalize_yz(subtract(current, wing.xsecs[section_index - 1].xyz_le))
            .ok_or_else(failure)?;
        let after = normalize_yz(subtract(wing.xsecs[section_index + 1].xyz_le, current))
            .ok_or_else(failure)?;
        [0.0, before[1] + after[1], before[2] + after[2]]
    };
    let direction = normalize_yz(direction).ok_or_else(failure)?;
    Ok(direction[2].atan2(direction[1]).to_degrees())
}

fn normalize_yz(vector: [f64; 3]) -> Option<[f64; 3]> {
    let magnitude = vector[1].hypot(vector[2]);
    (magnitude > 1.0e-12).then(|| [0.0, vector[1] / magnitude, vector[2] / magnitude])
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

pub(super) fn is_nacelle(fuselage: &Fuselage) -> bool {
    fuselage.name.to_ascii_lowercase().contains("nacelle")
}

fn wing_uid(index: usize) -> String {
    format!("alas-wing-{index}")
}

fn wing_section_uid(wing_index: usize, section_index: usize) -> String {
    format!("alas-wing-{wing_index}-section-{section_index}")
}

fn wing_element_uid(wing_index: usize, section_index: usize) -> String {
    format!("alas-wing-{wing_index}-element-{section_index}")
}

fn wing_segment_uid(wing_index: usize, segment_index: usize) -> String {
    format!("alas-wing-{wing_index}-segment-{segment_index}")
}

fn wing_airfoil_uid(wing_index: usize, section_index: usize) -> String {
    format!("alas-wing-{wing_index}-airfoil-{section_index}")
}

fn fuselage_uid(index: usize) -> String {
    format!("alas-fuselage-{index}")
}

fn fuselage_section_uid(fuselage_index: usize, section_index: usize) -> String {
    format!("alas-fuselage-{fuselage_index}-section-{section_index}")
}

fn fuselage_element_uid(fuselage_index: usize, section_index: usize) -> String {
    format!("alas-fuselage-{fuselage_index}-element-{section_index}")
}

fn fuselage_segment_uid(fuselage_index: usize, segment_index: usize) -> String {
    format!("alas-fuselage-{fuselage_index}-segment-{segment_index}")
}
