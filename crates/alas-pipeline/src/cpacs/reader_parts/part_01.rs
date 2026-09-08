// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::fs;
use std::path::Path;

use roxmltree::Node;

use super::model::{
    CpacsAircraft, CpacsDocument, CpacsEngine, CpacsEnginePosition, CpacsFuselage,
    CpacsFuselageElement, CpacsFuselageProfile, CpacsFuselageSection, CpacsHeader, CpacsReference,
    CpacsSegment, CpacsTransformation, CpacsTurboprop, CpacsVersionInfo, CpacsWing,
    CpacsWingAirfoil, CpacsWingElement, CpacsWingSection, CPACS_35_VERSION,
};

#[path = "../read_error.rs"]
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
            turboprop: None,
        });
    }
    Ok(engines)
}

fn parse_turboprop_extension(
    root: Node<'_, '_>,
) -> Result<Option<(String, CpacsTurboprop)>, CpacsReadError> {
    let Some(node) = root
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "propulsion")
    else {
        return Ok(None);
    };
    if optional_text(
        node,
        "technology",
        "cpacs/toolspecific/propulsion/technology",
    )?
    .as_deref()
        != Some("turboprop")
    {
        return Ok(None);
    }
    let path = "cpacs/toolspecific/propulsion";
    let number = |name: &str| required_number(node, name, &format!("{path}/{name}"));
    Ok(Some((
        required_text(node, "engineUID", &format!("{path}/engineUID"))?,
        CpacsTurboprop {
            propeller_model: required_text(
                node,
                "propellerModel",
                &format!("{path}/propellerModel"),
            )?,
            takeoff_shaft_power_kw: number("takeoffShaftPowerKW")?,
            maximum_reserve_shaft_power_kw: number("maximumReserveShaftPowerKW")?,
            maximum_continuous_shaft_power_kw: number("maximumContinuousShaftPowerKW")?,
            maximum_climb_shaft_power_kw: number("maximumClimbShaftPowerKW")?,
            maximum_cruise_shaft_power_kw: number("maximumCruiseShaftPowerKW")?,
            maximum_cruise_fuel_flow_kg_h: number("maximumCruiseFuelFlowKgH")?,
            propeller_diameter_m: number("propellerDiameterM")?,
            governed_propeller_speed_rpm: number("governedPropellerSpeedRPM")?,
            reduction_ratio: number("reductionRatio")?,
        },
    )))
}
