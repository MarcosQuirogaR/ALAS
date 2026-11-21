// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main wing's reference wing and design planform, which CPACS 3.5 cannot
//! encode: a wing there is its lofted sections. Without them a reimported export
//! would read its masses, tanks, sweep and `%MAC` on the loft. The sections and the
//! document's own reference are recorded with them, so an import can tell whether the
//! record still describes the document. Metres, square metres and degrees, aircraft
//! axes, x aft.

use std::fmt::Write as FmtWrite;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{PlanformDatum, WingReference};

use super::{
    validate_number, validate_vector, write_number_element, write_point, write_text,
    CpacsExportError,
};

fn reference_of(airplane: &Airplane) -> Option<WingReference> {
    airplane.main_wing().and_then(|wing| wing.reference)
}

fn datum_of(airplane: &Airplane) -> Option<PlanformDatum> {
    airplane.main_wing().and_then(|wing| wing.planform_datum)
}

fn tip_te_flare_of(airplane: &Airplane) -> f64 {
    airplane.main_wing().map_or(0.0, |wing| wing.tip_te_flare_m)
}

/// Whether the main wing has anything [`write_main_wing_tool`] would record.
pub(super) fn has_main_wing_record(airplane: &Airplane) -> bool {
    reference_of(airplane).is_some()
        || datum_of(airplane).is_some()
        || tip_te_flare_of(airplane) != 0.0
}

pub(super) fn validate_main_wing_record(airplane: &Airplane) -> Result<(), CpacsExportError> {
    validate_number(tip_te_flare_of(airplane), "wing tip trailing-edge flare")?;
    if let Some(reference) = reference_of(airplane) {
        for (value, context) in [
            (reference.area_m2, "reference wing area"),
            (reference.span_m, "reference wing span"),
            (reference.mac_m, "reference wing MAC"),
            (reference.mac_le_x_m, "reference wing MAC leading edge"),
        ] {
            validate_number(value, context)?;
        }
    }
    if let Some(datum) = datum_of(airplane) {
        validate_vector(datum.root_le_m, "design root leading edge")?;
        validate_vector(datum.root_chord_axis, "design root chord axis")?;
        for (value, context) in [
            (datum.root_chord_m, "design root chord"),
            (datum.root_twist_deg, "design root twist"),
            (datum.tip_le_x_m, "straight-edge tip leading edge"),
            (datum.tip_chord_m, "straight-edge tip chord"),
            (datum.gross_area_m2, "design planform area"),
            (datum.gross_mac_m, "design planform MAC"),
            (datum.gross_mac_le_x_m, "design planform MAC leading edge"),
            (datum.side_of_body_y_m, "side-of-body station"),
            (datum.side_of_body_le_x_m, "side-of-body leading edge"),
            (datum.side_of_body_chord_m, "side-of-body chord"),
            (datum.side_of_body_twist_deg, "side-of-body twist"),
        ] {
            validate_number(value, context)?;
        }
    }
    Ok(())
}

/// Emit the `<tool>` that records the main wing's reference and datum, when it
/// has either.
pub(super) fn write_main_wing_tool(xml: &mut String, airplane: &Airplane) {
    if !has_main_wing_record(airplane) {
        return;
    }
    let _ = writeln!(xml, "    <tool>");
    write_text(xml, 6, "name", "ALAS main wing");
    write_text(xml, 6, "version", "1.0");
    let _ = writeln!(
        xml,
        "      <alasMainWing xmlns=\"https://alas.dev/cpacs/main-wing/1.0\">"
    );
    if let Some(reference) = reference_of(airplane) {
        let _ = writeln!(xml, "        <referenceWing>");
        write_number_element(xml, 10, "areaM2", reference.area_m2);
        write_number_element(xml, 10, "spanM", reference.span_m);
        write_number_element(xml, 10, "macM", reference.mac_m);
        write_number_element(xml, 10, "macLeadingEdgeXM", reference.mac_le_x_m);
        let _ = writeln!(xml, "        </referenceWing>");
    }
    if let Some(datum) = datum_of(airplane) {
        let _ = writeln!(xml, "        <planformDatum>");
        write_point(xml, 10, "rootLeadingEdge", datum.root_le_m, None);
        write_number_element(xml, 10, "rootChordM", datum.root_chord_m);
        write_point(xml, 10, "rootChordAxis", datum.root_chord_axis, None);
        write_number_element(xml, 10, "rootTwistDeg", datum.root_twist_deg);
        write_number_element(xml, 10, "tipLeadingEdgeXM", datum.tip_le_x_m);
        write_number_element(xml, 10, "tipChordM", datum.tip_chord_m);
        write_number_element(xml, 10, "grossAreaM2", datum.gross_area_m2);
        write_number_element(xml, 10, "grossMacM", datum.gross_mac_m);
        write_number_element(xml, 10, "grossMacLeadingEdgeXM", datum.gross_mac_le_x_m);
        write_number_element(xml, 10, "sideOfBodyYM", datum.side_of_body_y_m);
        write_number_element(
            xml,
            10,
            "sideOfBodyLeadingEdgeXM",
            datum.side_of_body_le_x_m,
        );
        write_number_element(xml, 10, "sideOfBodyChordM", datum.side_of_body_chord_m);
        write_number_element(xml, 10, "sideOfBodyTwistDeg", datum.side_of_body_twist_deg);
        let _ = writeln!(xml, "        </planformDatum>");
    }
    let tip_te_flare_m = tip_te_flare_of(airplane);
    if tip_te_flare_m != 0.0 {
        write_number_element(xml, 8, "tipTrailingEdgeFlareM", tip_te_flare_m);
    }
    let _ = writeln!(xml, "        <documentReference>");
    write_number_element(xml, 10, "areaM2", airplane.s_ref);
    write_number_element(xml, 10, "lengthM", airplane.c_ref);
    let _ = writeln!(xml, "        </documentReference>");
    let _ = writeln!(xml, "        <sections>");
    for xsec in airplane.main_wing().map_or(&[][..], |wing| &wing.xsecs) {
        let _ = writeln!(xml, "          <section>");
        write_number_element(xml, 12, "xLeM", xsec.xyz_le[0]);
        write_number_element(xml, 12, "yM", xsec.xyz_le[1]);
        write_number_element(xml, 12, "zM", xsec.xyz_le[2]);
        write_number_element(xml, 12, "chordM", xsec.chord);
        write_number_element(xml, 12, "twistDeg", xsec.twist);
        let _ = writeln!(xml, "          </section>");
    }
    let _ = writeln!(xml, "        </sections>");
    let _ = writeln!(xml, "      </alasMainWing>");
    let _ = writeln!(xml, "    </tool>");
}
