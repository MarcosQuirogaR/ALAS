// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The bodies as the analyses read them, which the document's sections are not: those
//! carry the closed ends an external tool needs to see a closed solid, with no shape
//! exponent and with a scarfed lip upright. Without this a reimported export would
//! read its wetted areas and volumes on the closed surface. The lofted surface goes
//! with them as the loft data and instantiation it is built from, the APU cone as its
//! two stations, so an own export gives back the exact skin integrals. Metres,
//! aircraft axes, x aft.

use std::fmt::Write as FmtWrite;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::FuselageXSec;
use alas_geom::loft::Loft;

use super::{write_number_element, write_point, write_text};

/// Whether there is a body to record.
pub(super) fn has_body_record(airplane: &Airplane) -> bool {
    !airplane.fuselages.is_empty()
}

fn write_stations(xml: &mut String, element: &str, stations: &[FuselageXSec]) {
    let _ = writeln!(xml, "          <{element}>");
    for station in stations {
        let _ = writeln!(xml, "            <station>");
        write_number_element(xml, 14, "xM", station.xyz_c[0]);
        write_number_element(xml, 14, "yM", station.xyz_c[1]);
        write_number_element(xml, 14, "zM", station.xyz_c[2]);
        write_number_element(xml, 14, "widthM", station.width);
        write_number_element(xml, 14, "heightM", station.height);
        write_number_element(xml, 14, "shape", station.shape);
        let _ = writeln!(xml, "            </station>");
    }
    let _ = writeln!(xml, "          </{element}>");
}

/// The loft data as the data-file schema writes it, without what does not shape the
/// surface. The shortest decimal text of each number reads back to the same bits.
fn write_loft(xml: &mut String, loft: &Loft) {
    let mut spec = loft.spec().clone();
    spec.frame = String::new();
    spec.fit = alas_config::loft::FitRecord::default();
    spec.provenance = Vec::new();
    let Ok(json) = serde_json::to_string(&spec) else {
        return;
    };
    let params = loft.params();
    let _ = writeln!(xml, "          <loft>");
    write_text(xml, 12, "specJson", &json);
    write_number_element(xml, 12, "lengthM", params.length_m);
    write_number_element(xml, 12, "widthScale", params.width_scale);
    write_number_element(xml, 12, "heightScale", params.height_scale);
    write_number_element(xml, 12, "noseScale", params.nose_scale);
    write_number_element(xml, 12, "tailScale", params.tail_scale);
    write_point(xml, 12, "origin", params.origin, None);
    let _ = writeln!(xml, "          </loft>");
}

/// Emit the `<tool>` that records the bodies, when there is any.
pub(super) fn write_body_record_tool(xml: &mut String, airplane: &Airplane) {
    if !has_body_record(airplane) {
        return;
    }
    let _ = writeln!(xml, "    <tool>");
    write_text(xml, 6, "name", "ALAS body sections");
    write_text(xml, 6, "version", "1.0");
    let _ = writeln!(
        xml,
        "      <alasBodySections xmlns=\"https://alas.dev/cpacs/body-sections/1.0\">"
    );
    for body in &airplane.fuselages {
        let _ = writeln!(xml, "        <body>");
        write_text(xml, 10, "bodyName", &body.name);
        write_stations(xml, "analysisStations", &body.xsecs);
        if let Some(scarf) = &body.lip_scarf {
            let _ = writeln!(xml, "          <lipScarf>");
            write_number_element(xml, 12, "highlightXM", scarf.highlight_x_m);
            write_number_element(xml, 12, "blendM", scarf.blend_m);
            write_number_element(xml, 12, "slope", scarf.slope);
            let _ = writeln!(xml, "          </lipScarf>");
        }
        if let Some(loft) = &body.loft {
            write_loft(xml, loft);
        }
        if let Some(cone) = &body.apu_cone {
            write_stations(xml, "apuConeStations", &cone.xsecs);
        }
        let _ = writeln!(xml, "        </body>");
    }
    let _ = writeln!(xml, "      </alasBodySections>");
    let _ = writeln!(xml, "    </tool>");
}
