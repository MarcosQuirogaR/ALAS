// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! CPACS 3.5 XML reader and reference validation.
//!
//! The reader is a structural boundary. It keeps the values written by the
//! current exporter, including SI coordinates, transformations, symmetry and
//! UIDs, then validates references before returning the typed document.

mod airframe;
mod engines;
mod read_error;
mod xml;

use airframe::*;
use engines::*;
pub use read_error::CpacsReadError;
use xml::*;

use std::fs;
use std::path::Path;

use roxmltree::Node;

use super::model::{
    CpacsAircraft, CpacsDocument, CpacsEngine, CpacsEnginePosition, CpacsFuselage,
    CpacsFuselageElement, CpacsFuselageProfile, CpacsFuselageSection, CpacsHeader, CpacsReference,
    CpacsSegment, CpacsTransformation, CpacsTurboprop, CpacsVersionInfo, CpacsWing,
    CpacsWingAirfoil, CpacsWingElement, CpacsWingSection, CPACS_35_VERSION,
};

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
    let mut engines = parse_engines(vehicles)?;
    if let Some((engine_uid, turboprop)) = parse_turboprop_extension(root)? {
        if let Some(engine) = engines.iter_mut().find(|engine| engine.uid == engine_uid) {
            engine.turboprop = Some(turboprop);
        }
    }
    let document = CpacsDocument {
        cpacs_version,
        header,
        aircraft: parse_aircraft(model)?,
        engines,
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
pub(super) fn parse_header<'a, 'input: 'a>(
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
