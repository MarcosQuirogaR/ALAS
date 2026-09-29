// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Numbers, points, transformations and attributes read off XML nodes.

use super::*;

pub(super) fn optional_nested_number<'a, 'input: 'a>(
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

pub(super) fn optional_transformation<'a, 'input: 'a>(
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

pub(super) fn optional_point<'a, 'input: 'a>(
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

pub(super) fn optional_number<'a, 'input: 'a>(
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

pub(super) fn required_number<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<f64, CpacsReadError> {
    optional_number(node, name, path)?.ok_or_else(|| CpacsReadError::MissingElement {
        path: path.to_owned(),
    })
}

pub(super) fn parse_number(value: &str, path: &str) -> Result<f64, CpacsReadError> {
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

pub(super) fn required_child<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
    path: &str,
) -> Result<Node<'a, 'input>, CpacsReadError> {
    direct_child(node, name).ok_or_else(|| CpacsReadError::MissingElement {
        path: path.to_owned(),
    })
}

pub(super) fn direct_child<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.has_tag_name(name))
}

pub(super) fn element_children<'a, 'input: 'a>(
    node: Node<'a, 'input>,
    name: &str,
) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|child| child.is_element() && child.has_tag_name(name))
        .collect()
}

pub(super) fn required_uid<'a, 'input: 'a>(
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

pub(super) fn optional_attribute<'a, 'input: 'a>(
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

pub(super) fn required_text<'a, 'input: 'a>(
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

pub(super) fn optional_text<'a, 'input: 'a>(
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
