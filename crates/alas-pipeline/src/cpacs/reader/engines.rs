// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Engine positions and the turboprop extension.

use super::*;

pub(super) fn parse_engine_position<'a, 'input: 'a>(
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

pub(super) fn parse_engines<'a, 'input: 'a>(
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

pub(super) fn parse_turboprop_extension(
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
