// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS toolspecific records of the tail reference planforms.

use super::super::model::{CpacsRecordedSection, CpacsTailPlanformRecord};
use super::*;

/// The records of an ALAS export, empty for any other document.
pub(super) fn parse_tail_planform_records(
    root: Node<'_, '_>,
) -> Result<Vec<CpacsTailPlanformRecord>, CpacsReadError> {
    let Some(node) = root
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "alasTailPlanforms")
    else {
        return Ok(Vec::new());
    };
    node.children()
        .filter(|child| child.is_element() && child.tag_name().name() == "surface")
        .enumerate()
        .map(|(index, surface)| {
            let path = format!("cpacs/toolspecific/alasTailPlanforms/surface[{index}]");
            Ok(CpacsTailPlanformRecord {
                name: required_text(surface, "surfaceName", &format!("{path}/surfaceName"))?,
                reference: parse_sections(surface, "referencePlanform", &path)?,
                sections: parse_sections(surface, "sections", &path)?,
            })
        })
        .collect()
}

fn parse_sections(
    surface: Node<'_, '_>,
    element: &str,
    path: &str,
) -> Result<Vec<CpacsRecordedSection>, CpacsReadError> {
    let Some(list) = direct_child(surface, element) else {
        return Ok(Vec::new());
    };
    list.children()
        .filter(|section| section.is_element() && section.tag_name().name() == "section")
        .enumerate()
        .map(|(index, section)| {
            let path = format!("{path}/{element}/section[{index}]");
            let number = |name: &str| required_number(section, name, &format!("{path}/{name}"));
            Ok(CpacsRecordedSection {
                le_m: [number("xLeM")?, number("yM")?, number("zM")?],
                chord_m: number("chordM")?,
                twist_deg: number("twistDeg")?,
            })
        })
        .collect()
}
