// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The design vector the exported aircraft was built from, which CPACS 3.5
//! has no element for. Without it a reimported export is analysed against the
//! configured vector, so a redesigned registered aircraft reads as the
//! unchanged preset: its tank inventory took the published capacity in place
//! of its own wing's, and the reported vector was the preset's. Variable
//! names and units are those of the design-space table.

use std::fmt::Write as FmtWrite;

use alas_config::design_variables::SPECS;
use alas_config::DesignVector;

use super::{validate_number, write_number_element, write_text, CpacsExportError};

pub(super) fn validate_design_vector_record(design: &DesignVector) -> Result<(), CpacsExportError> {
    for (spec, value) in SPECS.iter().zip(design.to_array()) {
        validate_number(value, spec.name)?;
    }
    if let Some(mtow_kg) = design.mtow_kg {
        validate_number(mtow_kg, "design takeoff mass")?;
    }
    Ok(())
}

pub(super) fn write_design_vector_tool(xml: &mut String, design: &DesignVector) {
    let _ = writeln!(xml, "    <tool>");
    write_text(xml, 6, "name", "ALAS design vector");
    write_text(xml, 6, "version", "1.0");
    let _ = writeln!(
        xml,
        "      <alasDesignVector xmlns=\"https://alas.dev/cpacs/design-vector/1.0\">"
    );
    for (spec, value) in SPECS.iter().zip(design.to_array()) {
        write_number_element(xml, 8, spec.name, value);
    }
    if let Some(mtow_kg) = design.mtow_kg {
        write_number_element(xml, 8, "mtow_kg", mtow_kg);
    }
    let _ = writeln!(xml, "      </alasDesignVector>");
    let _ = writeln!(xml, "    </tool>");
}
