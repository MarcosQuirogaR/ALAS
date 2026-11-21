// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS toolspecific record of the main wing's reference and design planform.

use alas_geom::aircraft::wing::{PlanformDatum, WingReference};

use super::super::model::{CpacsMainWingRecord, CpacsRecordedSection};
use super::*;

/// The record of an ALAS export, `None` for any other document.
pub(super) fn parse_main_wing_record(
    root: Node<'_, '_>,
) -> Result<Option<CpacsMainWingRecord>, CpacsReadError> {
    let Some(node) = root
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "alasMainWing")
    else {
        return Ok(None);
    };
    let path = "cpacs/toolspecific/alasMainWing";
    let reference = direct_child(node, "referenceWing")
        .map(|child| parse_reference_wing(child, &format!("{path}/referenceWing")))
        .transpose()?;
    let datum = direct_child(node, "planformDatum")
        .map(|child| parse_datum(child, &format!("{path}/planformDatum")))
        .transpose()?;
    let sections = direct_child(node, "sections")
        .map(|child| {
            child
                .children()
                .filter(|section| section.is_element() && section.tag_name().name() == "section")
                .enumerate()
                .map(|(index, section)| {
                    parse_section(section, &format!("{path}/sections/section[{index}]"))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let document_reference = direct_child(node, "documentReference")
        .map(|child| {
            let element = format!("{path}/documentReference");
            Ok::<_, CpacsReadError>((
                required_number(child, "areaM2", &format!("{element}/areaM2"))?,
                required_number(child, "lengthM", &format!("{element}/lengthM"))?,
            ))
        })
        .transpose()?;
    let tip_te_flare_m = optional_number(
        node,
        "tipTrailingEdgeFlareM",
        &format!("{path}/tipTrailingEdgeFlareM"),
    )?
    .unwrap_or(0.0);
    Ok(Some(CpacsMainWingRecord {
        reference,
        datum,
        tip_te_flare_m,
        sections,
        document_reference,
    }))
}

fn parse_section(node: Node<'_, '_>, path: &str) -> Result<CpacsRecordedSection, CpacsReadError> {
    let number = |name: &str| required_number(node, name, &format!("{path}/{name}"));
    Ok(CpacsRecordedSection {
        le_m: [number("xLeM")?, number("yM")?, number("zM")?],
        chord_m: number("chordM")?,
        twist_deg: number("twistDeg")?,
    })
}

fn parse_reference_wing(node: Node<'_, '_>, path: &str) -> Result<WingReference, CpacsReadError> {
    let number = |name: &str| required_number(node, name, &format!("{path}/{name}"));
    Ok(WingReference {
        area_m2: number("areaM2")?,
        span_m: number("spanM")?,
        mac_m: number("macM")?,
        mac_le_x_m: number("macLeadingEdgeXM")?,
    })
}

fn parse_datum(node: Node<'_, '_>, path: &str) -> Result<PlanformDatum, CpacsReadError> {
    let number = |name: &str| required_number(node, name, &format!("{path}/{name}"));
    let point = |name: &str| {
        let element = format!("{path}/{name}");
        optional_point(node, name, &element)?
            .ok_or(CpacsReadError::MissingElement { path: element })
    };
    Ok(PlanformDatum {
        root_le_m: point("rootLeadingEdge")?,
        root_chord_m: number("rootChordM")?,
        root_chord_axis: point("rootChordAxis")?,
        root_twist_deg: number("rootTwistDeg")?,
        tip_le_x_m: number("tipLeadingEdgeXM")?,
        tip_chord_m: number("tipChordM")?,
        gross_area_m2: number("grossAreaM2")?,
        gross_mac_m: number("grossMacM")?,
        gross_mac_le_x_m: number("grossMacLeadingEdgeXM")?,
        side_of_body_y_m: number("sideOfBodyYM")?,
        side_of_body_le_x_m: number("sideOfBodyLeadingEdgeXM")?,
        side_of_body_chord_m: number("sideOfBodyChordM")?,
        side_of_body_twist_deg: number("sideOfBodyTwistDeg")?,
    })
}
