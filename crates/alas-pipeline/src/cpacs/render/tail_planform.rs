// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The straight reference planform of the tail surfaces, which CPACS 3.5 cannot
//! encode: a wing there is its lofted sections. Without it a reimported export would
//! read its tail areas, tail volumes, FLOPS tail terms and mission trapezoid on the
//! built outline. The built sections are recorded with it, so an import can tell
//! whether the record still describes the document. Metres and degrees, aircraft
//! axes, x aft.

use std::fmt::Write as FmtWrite;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::WingXSec;

use super::{write_number_element, write_text};

/// Whether any surface has a reference planform of its own to record.
pub(super) fn has_tail_planform_record(airplane: &Airplane) -> bool {
    airplane
        .wings
        .iter()
        .any(|wing| wing.reference_xsecs.is_some())
}

fn write_sections(xml: &mut String, element: &str, sections: &[WingXSec]) {
    let _ = writeln!(xml, "          <{element}>");
    for xsec in sections {
        let _ = writeln!(xml, "            <section>");
        write_number_element(xml, 14, "xLeM", xsec.xyz_le[0]);
        write_number_element(xml, 14, "yM", xsec.xyz_le[1]);
        write_number_element(xml, 14, "zM", xsec.xyz_le[2]);
        write_number_element(xml, 14, "chordM", xsec.chord);
        write_number_element(xml, 14, "twistDeg", xsec.twist);
        let _ = writeln!(xml, "            </section>");
    }
    let _ = writeln!(xml, "          </{element}>");
}

/// Emit the `<tool>` that records the reference planforms, when there is any.
pub(super) fn write_tail_planform_tool(xml: &mut String, airplane: &Airplane) {
    if !has_tail_planform_record(airplane) {
        return;
    }
    let _ = writeln!(xml, "    <tool>");
    write_text(xml, 6, "name", "ALAS tail planforms");
    write_text(xml, 6, "version", "1.0");
    let _ = writeln!(
        xml,
        "      <alasTailPlanforms xmlns=\"https://alas.dev/cpacs/tail-planforms/1.0\">"
    );
    for wing in &airplane.wings {
        let Some(reference) = &wing.reference_xsecs else {
            continue;
        };
        let _ = writeln!(xml, "        <surface>");
        write_text(xml, 10, "surfaceName", &wing.name);
        write_sections(xml, "referencePlanform", reference);
        write_sections(xml, "sections", &wing.xsecs);
        let _ = writeln!(xml, "        </surface>");
    }
    let _ = writeln!(xml, "      </alasTailPlanforms>");
    let _ = writeln!(xml, "    </tool>");
}
