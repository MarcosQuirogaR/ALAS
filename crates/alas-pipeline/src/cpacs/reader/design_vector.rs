// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ALAS toolspecific record of the design vector an exported aircraft was
//! built from.

use alas_config::design_variables::SPECS;
use alas_config::DesignVector;

use super::*;

/// The recorded vector of an ALAS export, `None` for any other document.
pub(super) fn parse_design_vector_record(
    root: Node<'_, '_>,
) -> Result<Option<DesignVector>, CpacsReadError> {
    let Some(node) = root
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "alasDesignVector")
    else {
        return Ok(None);
    };
    let path = "cpacs/toolspecific/alasDesignVector";
    let values = SPECS
        .iter()
        .map(|spec| required_number(node, spec.name, &format!("{path}/{}", spec.name)))
        .collect::<Result<Vec<_>, _>>()?;
    let mtow_kg = optional_number(node, "mtow_kg", &format!("{path}/mtow_kg"))?;
    let design = DesignVector::from_array(&values).map_err(|_| CpacsReadError::MissingElement {
        path: path.to_owned(),
    })?;
    Ok(Some(DesignVector { mtow_kg, ..design }))
}
