// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuselage, wing and section elements of the CPACS aircraft.

use super::*;

pub(super) fn parse_fuselage<'a, 'input: 'a>(
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

pub(super) fn parse_wing<'a, 'input: 'a>(
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

pub(super) fn parse_fuselage_profiles<'a, 'input: 'a>(
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

pub(super) fn parse_wing_airfoils<'a, 'input: 'a>(
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
