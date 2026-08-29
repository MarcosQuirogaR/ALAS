// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 XML reader and reference validation.
//!
//! The reader is a structural boundary. It keeps the values written by the
//! current exporter, including SI coordinates, transformations, symmetry and
//! UIDs, then validates references before returning the typed document.

use std::fs;
use std::path::Path;

use roxmltree::Node;

use super::model::{
    CpacsAircraft, CpacsDocument, CpacsEngine, CpacsEnginePosition, CpacsFuselage,
    CpacsFuselageElement, CpacsFuselageProfile, CpacsFuselageSection, CpacsHeader, CpacsReference,
    CpacsSegment, CpacsTransformation, CpacsVersionInfo, CpacsWing, CpacsWingAirfoil,
    CpacsWingElement, CpacsWingSection, CPACS_35_VERSION,
};

#[path = "read_error.rs"]
mod read_error;
pub use read_error::CpacsReadError;

/// Read and validate a CPACS 3.5 XML document from a string.
pub fn read_cpacs(xml: &str) -> Result<CpacsDocument, CpacsReadError> {
    let xml_document = roxmltree::Document::parse(xml)?;
    let root = xml_document.root_element();
    if !root.has_tag_name("cpacs") {
        return Err(CpacsReadError::InvalidRoot {
            found: root.tag_name().name().to_owned(),
        });
    }

    let (header, cpacs_version) = parse_header(root)?;
    let vehicles = required_child(root, "vehicles", "cpacs/vehicles")?;
    let aircraft = required_child(vehicles, "aircraft", "cpacs/vehicles/aircraft")?;
    let model = required_child(aircraft, "model", "cpacs/vehicles/aircraft/model")?;
    let document = CpacsDocument {
        cpacs_version,
        header,
        aircraft: parse_aircraft(model)?,
        engines: parse_engines(vehicles)?,
        fuselage_profiles: parse_fuselage_profiles(vehicles)?,
        wing_airfoils: parse_wing_airfoils(vehicles)?,
    };
    super::validation::validate_document(&document)?;
    Ok(document)
}

/// Read and validate a CPACS 3.5 XML document from a file.
pub fn read_cpacs_file(path: impl AsRef<Path>) -> Result<CpacsDocument, CpacsReadError> {
    let xml = fs::read_to_string(path)?;
    read_cpacs(&xml)
}
fn parse_header<'a, 'input: 'a>(
    root: Node<'a, 'input>,
) -> Result<(CpacsHeader, String), CpacsReadError> {
    let node = required_child(root, "header", "cpacs/header")?;
    let name = optional_text(node, "name", "cpacs/header/name")?;
    let description = optional_text(node, "description", "cpacs/header/description")?;
    let version = optional_text(node, "version", "cpacs/header/version")?;
    let direct_version = optional_text(node, "cpacsVersion", "cpacs/header/cpacsVersion")?;
    let mut version_infos = Vec::new();
    if let Some(container) = direct_child(node, "versionInfos") {
        for (index, child) in element_children(container, "versionInfo")
            .into_iter()
            .enumerate()
        {
            let path = format!("cpacs/header/versionInfos/versionInfo[{index}]");
            version_infos.push(CpacsVersionInfo {
                version: optional_attribute(child, "version", &path)?,
                cpacs_version: optional_text(
                    child,
                    "cpacsVersion",
                    &format!("{path}/cpacsVersion"),
                )?,
                description: optional_text(child, "description", &format!("{path}/description"))?,
                timestamp: optional_text(child, "timestamp", &format!("{path}/timestamp"))?,
                creator: optional_text(child, "creator", &format!("{path}/creator"))?,
            });
        }
    }

    let selected = version.as_deref().and_then(|header_version| {
        version_infos
            .iter()
            .find(|info| info.version.as_deref() == Some(header_version))
            .and_then(|info| info.cpacs_version.clone())
    });
    let declared = selected
        .or_else(|| {
            version_infos
                .iter()
                .rev()
                .find_map(|info| info.cpacs_version.clone())
        })
        .or(direct_version)
        .ok_or(CpacsReadError::MissingVersion)?;
    if declared != CPACS_35_VERSION {
        return Err(CpacsReadError::UnsupportedVersion { found: declared });
    }
    Ok((
        CpacsHeader {
            name,
            description,
            version,
            version_infos,
        },
        CPACS_35_VERSION.to_owned(),
    ))
}

fn parse_aircraft<'a, 'input: 'a>(node: Node<'a, 'input>) -> Result<CpacsAircraft, CpacsReadError> {
    let path = "cpacs/vehicles/aircraft/model";
    let mut fuselages = Vec::new();
    if let Some(container) = direct_child(node, "fuselages") {
        for (index, child) in element_children(container, "fuselage")
            .into_iter()
            .enumerate()
        {
            fuselages.push(parse_fuselage(
                child,
                &format!("{path}/fuselages/fuselage[{index}]"),
            )?);
        }
    }
    let mut wings = Vec::new();
    if let Some(container) = direct_child(node, "wings") {
        for (index, child) in element_children(container, "wing").into_iter().enumerate() {
            wings.push(parse_wing(child, &format!("{path}/wings/wing[{index}]"))?);
        }
    }
    let mut engine_positions = Vec::new();
    if let Some(container) = direct_child(node, "engines") {
        for (index, child) in element_children(container, "engine")
            .into_iter()
            .enumerate()
        {
            engine_positions.push(parse_engine_position(
                child,
                &format!("{path}/engines/engine[{index}]"),
            )?);
        }
    }
    Ok(CpacsAircraft {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        description: optional_text(node, "description", &format!("{path}/description"))?,
        reference: direct_child(node, "reference")
            .map(|child| parse_reference(child, &format!("{path}/reference")))
            .transpose()?,
        fuselages,
        wings,
        engine_positions,
    })
}
fn parse_reference<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsReference, CpacsReadError> {
    Ok(CpacsReference {
        area: optional_number(node, "area", &format!("{path}/area"))?,
        length: optional_number(node, "length", &format!("{path}/length"))?,
        point: optional_point(node, "point", &format!("{path}/point"))?,
    })
}
fn parse_fuselage<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsFuselage, CpacsReadError> {
    let mut sections = Vec::new();
    if let Some(container) = direct_child(node, "sections") {
        for (index, child) in element_children(container, "section")
            .into_iter()
            .enumerate()
        {
            sections.push(parse_fuselage_section(
                child,
                &format!("{path}/sections/section[{index}]"),
            )?);
        }
    }
    let mut segments = Vec::new();
    if let Some(container) = direct_child(node, "segments") {
        for (index, child) in element_children(container, "segment")
            .into_iter()
            .enumerate()
        {
            segments.push(parse_segment(
                child,
                &format!("{path}/segments/segment[{index}]"),
            )?);
        }
    }
    Ok(CpacsFuselage {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        description: optional_text(node, "description", &format!("{path}/description"))?,
        parent_uid: optional_text(node, "parentUID", &format!("{path}/parentUID"))?,
        transformation: optional_transformation(node, path)?,
        sections,
        segments,
    })
}

fn parse_fuselage_section<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsFuselageSection, CpacsReadError> {
    let mut elements = Vec::new();
    if let Some(container) = direct_child(node, "elements") {
        for (index, child) in element_children(container, "element")
            .into_iter()
            .enumerate()
        {
            elements.push(parse_fuselage_element(
                child,
                &format!("{path}/elements/element[{index}]"),
            )?);
        }
    }
    Ok(CpacsFuselageSection {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        transformation: optional_transformation(node, path)?,
        elements,
    })
}

fn parse_fuselage_element<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsFuselageElement, CpacsReadError> {
    Ok(CpacsFuselageElement {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        profile_uid: required_text(node, "profileUID", &format!("{path}/profileUID"))?,
        transformation: optional_transformation(node, path)?,
    })
}

fn parse_wing<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsWing, CpacsReadError> {
    let mut sections = Vec::new();
    if let Some(container) = direct_child(node, "sections") {
        for (index, child) in element_children(container, "section")
            .into_iter()
            .enumerate()
        {
            sections.push(parse_wing_section(
                child,
                &format!("{path}/sections/section[{index}]"),
            )?);
        }
    }
    let mut segments = Vec::new();
    if let Some(container) = direct_child(node, "segments") {
        for (index, child) in element_children(container, "segment")
            .into_iter()
            .enumerate()
        {
            segments.push(parse_segment(
                child,
                &format!("{path}/segments/segment[{index}]"),
            )?);
        }
    }
    Ok(CpacsWing {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        description: optional_text(node, "description", &format!("{path}/description"))?,
        parent_uid: optional_text(node, "parentUID", &format!("{path}/parentUID"))?,
        symmetry: optional_attribute(node, "symmetry", path)?,
        transformation: optional_transformation(node, path)?,
        sections,
        segments,
    })
}

fn parse_wing_section<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsWingSection, CpacsReadError> {
    let mut elements = Vec::new();
    if let Some(container) = direct_child(node, "elements") {
        for (index, child) in element_children(container, "element")
            .into_iter()
            .enumerate()
        {
            elements.push(parse_wing_element(
                child,
                &format!("{path}/elements/element[{index}]"),
            )?);
        }
    }
    Ok(CpacsWingSection {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        transformation: optional_transformation(node, path)?,
        elements,
    })
}

fn parse_wing_element<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsWingElement, CpacsReadError> {
    Ok(CpacsWingElement {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        airfoil_uid: required_text(node, "airfoilUID", &format!("{path}/airfoilUID"))?,
        transformation: optional_transformation(node, path)?,
    })
}

fn parse_segment<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsSegment, CpacsReadError> {
    Ok(CpacsSegment {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        from_element_uid: required_text(node, "fromElementUID", &format!("{path}/fromElementUID"))?,
        to_element_uid: required_text(node, "toElementUID", &format!("{path}/toElementUID"))?,
    })
}

fn parse_engine_position<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsEnginePosition, CpacsReadError> {
    Ok(CpacsEnginePosition {
        uid: required_uid(node, path)?,
        name: required_text(node, "name", &format!("{path}/name"))?,
        engine_uid: required_text(node, "engineUID", &format!("{path}/engineUID"))?,
        parent_uid: required_text(node, "parentUID", &format!("{path}/parentUID"))?,
        transformation: optional_transformation(node, path)?,
    })
}

fn parse_engines<'a, 'input: 'a>(
    vehicles: Node<'a, 'input>,
) -> Result<Vec<CpacsEngine>, CpacsReadError> {
    let Some(container) = direct_child(vehicles, "engines") else {
        return Ok(Vec::new());
    };
    let mut engines = Vec::new();
    for (index, node) in element_children(container, "engine")
        .into_iter()
        .enumerate()
    {
        let path = format!("cpacs/vehicles/engines/engine[{index}]");
        engines.push(CpacsEngine {
            uid: required_uid(node, &path)?,
            name: required_text(node, "name", &format!("{path}/name"))?,
            description: optional_text(node, "description", &format!("{path}/description"))?,
            geometry_length_m: optional_nested_number(
                node,
                "geometry",
                "length",
                &format!("{path}/geometry/length"),
            )?,
            geometry_diameter_m: optional_nested_number(
                node,
                "geometry",
                "diameter",
                &format!("{path}/geometry/diameter"),
            )?,
            thrust00_n: optional_nested_number(
                node,
                "analysis",
                "thrust00",
                &format!("{path}/analysis/thrust00"),
            )?,
            fpr00: optional_nested_number(
                node,
                "analysis",
                "fpr00",
                &format!("{path}/analysis/fpr00"),
            )?,
            bpr00: optional_nested_number(
                node,
                "analysis",
                "bpr00",
                &format!("{path}/analysis/bpr00"),
            )?,
            opr00: optional_nested_number(
                node,
                "analysis",
                "opr00",
                &format!("{path}/analysis/opr00"),
            )?,
        });
    }
    Ok(engines)
}

fn optional_nested_number<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    container_name: &str,
    value_name: &str,
    path: &str,
) -> Result<Option<f64>, CpacsReadError> {
    direct_child(node, container_name)
        .map(|container| optional_number(container, value_name, path))
        .transpose()
        .map(Option::flatten)
}

fn parse_fuselage_profiles<'a, 'input: 'a>(
    vehicles: Node<'a, 'input>,
) -> Result<Vec<CpacsFuselageProfile>, CpacsReadError> {
    let Some(profiles) = direct_child(vehicles, "profiles") else {
        return Ok(Vec::new());
    };
    let Some(container) = direct_child(profiles, "fuselageProfiles") else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for (index, node) in element_children(container, "fuselageProfile")
        .into_iter()
        .enumerate()
    {
        let path = format!("cpacs/vehicles/profiles/fuselageProfiles/fuselageProfile[{index}]");
        let (map_type, points) = parse_point_list(node, &path)?;
        result.push(CpacsFuselageProfile {
            uid: required_uid(node, &path)?,
            name: required_text(node, "name", &format!("{path}/name"))?,
            description: optional_text(node, "description", &format!("{path}/description"))?,
            map_type,
            points,
        });
    }
    Ok(result)
}

fn parse_wing_airfoils<'a, 'input: 'a>(
    vehicles: Node<'a, 'input>,
) -> Result<Vec<CpacsWingAirfoil>, CpacsReadError> {
    let Some(profiles) = direct_child(vehicles, "profiles") else {
        return Ok(Vec::new());
    };
    let Some(container) = direct_child(profiles, "wingAirfoils") else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for (index, node) in element_children(container, "wingAirfoil")
        .into_iter()
        .enumerate()
    {
        let path = format!("cpacs/vehicles/profiles/wingAirfoils/wingAirfoil[{index}]");
        let (map_type, points) = parse_point_list(node, &path)?;
        result.push(CpacsWingAirfoil {
            uid: required_uid(node, &path)?,
            name: required_text(node, "name", &format!("{path}/name"))?,
            description: optional_text(node, "description", &format!("{path}/description"))?,
            map_type,
            points,
        });
    }
    Ok(result)
}

fn parse_point_list<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<(Option<String>, Vec<[f64; 3]>), CpacsReadError> {
    let point_list = required_child(node, "pointList", &format!("{path}/pointList"))?;
    let point_path = format!("{path}/pointList");
    let map_type = optional_attribute(point_list, "mapType", &point_path)?;
    let x = parse_vector_values(point_list, "x", &format!("{point_path}/x"))?;
    let y = parse_vector_values(point_list, "y", &format!("{point_path}/y"))?;
    let z = parse_vector_values(point_list, "z", &format!("{point_path}/z"))?;
    if x.len() != y.len() || x.len() != z.len() {
        return Err(CpacsReadError::InvalidPointList {
            path: point_path,
            reason: format!(
                "coordinate lengths are {}, {}, {}",
                x.len(),
                y.len(),
                z.len()
            ),
        });
    }
    let points = x
        .into_iter()
        .zip(y)
        .zip(z)
        .map(|((x_value, y_value), z_value)| [x_value, y_value, z_value])
        .collect();
    Ok((map_type, points))
}

fn parse_vector_values<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Vec<f64>, CpacsReadError> {
    let value = required_text(node, name, path)?;
    value
        .split(';')
        .enumerate()
        .map(|(index, value)| parse_number(value.trim(), &format!("{path}[{index}]")))
        .collect()
}

fn optional_transformation<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<Option<CpacsTransformation>, CpacsReadError> {
    direct_child(node, "transformation")
        .map(|child| parse_transformation(child, &format!("{path}/transformation")))
        .transpose()
}

fn parse_transformation<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<CpacsTransformation, CpacsReadError> {
    let translation = optional_point(node, "translation", &format!("{path}/translation"))?;
    let translation_reference = direct_child(node, "translation")
        .map(|child| optional_attribute(child, "refType", &format!("{path}/translation")))
        .transpose()?
        .flatten();
    Ok(CpacsTransformation {
        scaling: optional_point(node, "scaling", &format!("{path}/scaling"))?,
        rotation: optional_point(node, "rotation", &format!("{path}/rotation"))?,
        translation,
        translation_reference,
    })
}

fn optional_point<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Option<[f64; 3]>, CpacsReadError> {
    direct_child(node, name)
        .map(|child| parse_point(child, path))
        .transpose()
}

fn parse_point<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<[f64; 3], CpacsReadError> {
    let x = parse_number(
        &required_text(node, "x", &format!("{path}/x"))?,
        &format!("{path}/x"),
    )?;
    let y = parse_number(
        &required_text(node, "y", &format!("{path}/y"))?,
        &format!("{path}/y"),
    )?;
    let z = parse_number(
        &required_text(node, "z", &format!("{path}/z"))?,
        &format!("{path}/z"),
    )?;
    Ok([x, y, z])
}

fn optional_number<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Option<f64>, CpacsReadError> {
    direct_child(node, name)
        .map(|child| {
            let value = child.text().unwrap_or_default().trim().to_owned();
            if value.is_empty() {
                return Err(CpacsReadError::EmptyValue {
                    path: path.to_owned(),
                });
            }
            parse_number(&value, path)
        })
        .transpose()
}

fn parse_number(value: &str, path: &str) -> Result<f64, CpacsReadError> {
    let number = value
        .parse::<f64>()
        .map_err(|_| CpacsReadError::InvalidNumber {
            path: path.to_owned(),
            value: value.to_owned(),
        })?;
    if !number.is_finite() {
        return Err(CpacsReadError::InvalidNumber {
            path: path.to_owned(),
            value: value.to_owned(),
        });
    }
    Ok(number)
}

fn required_child<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Node<'a, 'input>, CpacsReadError> {
    direct_child(node, name).ok_or_else(|| CpacsReadError::MissingElement {
        path: path.to_owned(),
    })
}

fn direct_child<'a, 'input: 'a>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.has_tag_name(name))
}

fn element_children<'a, 'input: 'a>(node: Node<'a, 'input>, name: &str) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|child| child.is_element() && child.has_tag_name(name))
        .collect()
}

fn required_uid<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    path: &str,
) -> Result<String, CpacsReadError> {
    required_attribute(node, "uID", path)
}

fn required_attribute<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    attribute: &str,
    path: &str,
) -> Result<String, CpacsReadError> {
    let value = node
        .attribute(attribute)
        .ok_or_else(|| CpacsReadError::MissingAttribute {
            path: path.to_owned(),
            attribute: attribute.to_owned(),
        })?
        .trim();
    if value.is_empty() {
        return Err(CpacsReadError::EmptyValue {
            path: format!("{path}/@{attribute}"),
        });
    }
    Ok(value.to_owned())
}

fn optional_attribute<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    attribute: &str,
    path: &str,
) -> Result<Option<String>, CpacsReadError> {
    node.attribute(attribute)
        .map(|value| {
            let value = value.trim();
            if value.is_empty() {
                return Err(CpacsReadError::EmptyValue {
                    path: format!("{path}/@{attribute}"),
                });
            }
            Ok(value.to_owned())
        })
        .transpose()
}

fn required_text<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<String, CpacsReadError> {
    let child = required_child(node, name, path)?;
    let value = child.text().unwrap_or_default().trim();
    if value.is_empty() {
        return Err(CpacsReadError::EmptyValue {
            path: path.to_owned(),
        });
    }
    Ok(value.to_owned())
}

fn optional_text<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Option<String>, CpacsReadError> {
    direct_child(node, name)
        .map(|child| {
            let value = child.text().unwrap_or_default().trim();
            if value.is_empty() {
                return Err(CpacsReadError::EmptyValue {
                    path: path.to_owned(),
                });
            }
            Ok(value.to_owned())
        })
        .transpose()
}
