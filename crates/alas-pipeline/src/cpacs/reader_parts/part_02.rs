// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

fn required_number<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<f64, CpacsReadError> {
    optional_number(node, name, path)?.ok_or_else(|| CpacsReadError::MissingElement {
        path: path.to_owned(),
    })
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

