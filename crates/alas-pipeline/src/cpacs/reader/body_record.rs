// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS toolspecific records of the bodies as the analyses read them.

use alas_geom::aircraft::fuselage::{FuselageXSec, LipScarf};
use alas_geom::loft::{LoftParams, LoftSpec};

use super::super::model::{CpacsBodyLoft, CpacsBodyRecord};
use super::*;

/// The records of an ALAS export, empty for any other document.
pub(super) fn parse_body_records(
    root: Node<'_, '_>,
) -> Result<Vec<CpacsBodyRecord>, CpacsReadError> {
    let Some(node) = root
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "alasBodySections")
    else {
        return Ok(Vec::new());
    };
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == "body")
        .enumerate()
        .map(|(index, body)| {
            let path = format!("cpacs/toolspecific/alasBodySections/body[{index}]");
            Ok(CpacsBodyRecord {
                name: required_text(body, "bodyName", &format!("{path}/bodyName"))?,
                stations: parse_stations(body, "analysisStations", &path)?.unwrap_or_default(),
                lip_scarf: parse_lip_scarf(body, &path)?,
                loft: parse_loft(body, &path)?,
                apu_cone: parse_stations(body, "apuConeStations", &path)?,
            })
        })
        .collect()
}

fn parse_stations(
    body: Node<'_, '_>,
    element: &str,
    path: &str,
) -> Result<Option<Vec<FuselageXSec>>, CpacsReadError> {
    let Some(list) = direct_child(body, element) else {
        return Ok(None);
    };
    list.children()
        .filter(|station| station.is_element() && station.tag_name().name() == "station")
        .enumerate()
        .map(|(index, station)| {
            let path = format!("{path}/{element}/station[{index}]");
            let number = |name: &str| required_number(station, name, &format!("{path}/{name}"));
            Ok(FuselageXSec {
                xyz_c: [number("xM")?, number("yM")?, number("zM")?],
                width: number("widthM")?,
                height: number("heightM")?,
                shape: number("shape")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// `None` for a record with no loft, and for loft data this build cannot read, which
/// leaves the body on its stations.
fn parse_loft(body: Node<'_, '_>, path: &str) -> Result<Option<CpacsBodyLoft>, CpacsReadError> {
    let Some(loft) = direct_child(body, "loft") else {
        return Ok(None);
    };
    let path = format!("{path}/loft");
    let number = |name: &str| required_number(loft, name, &format!("{path}/{name}"));
    let json = required_text(loft, "specJson", &format!("{path}/specJson"))?;
    let origin = required_child(loft, "origin", &format!("{path}/origin"))?;
    let coordinate = |name: &str| required_number(origin, name, &format!("{path}/origin/{name}"));
    let params = LoftParams {
        length_m: number("lengthM")?,
        width_scale: number("widthScale")?,
        height_scale: number("heightScale")?,
        nose_scale: number("noseScale")?,
        tail_scale: number("tailScale")?,
        origin: [coordinate("x")?, coordinate("y")?, coordinate("z")?],
    };
    match serde_json::from_str::<LoftSpec>(&json) {
        Ok(spec) => Ok(Some(CpacsBodyLoft { spec, params })),
        Err(error) => {
            tracing::warn!(%error, "the loft data of an ALAS body record cannot be read and was ignored, so the body is read on its stations");
            Ok(None)
        }
    }
}

fn parse_lip_scarf(body: Node<'_, '_>, path: &str) -> Result<Option<LipScarf>, CpacsReadError> {
    let Some(scarf) = direct_child(body, "lipScarf") else {
        return Ok(None);
    };
    let number = |name: &str| required_number(scarf, name, &format!("{path}/lipScarf/{name}"));
    Ok(Some(LipScarf {
        highlight_x_m: number("highlightXM")?,
        blend_m: number("blendM")?,
        slope: number("slope")?,
    }))
}
